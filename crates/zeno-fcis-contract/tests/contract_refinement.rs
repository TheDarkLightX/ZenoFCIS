//! Semantic and malformed-input controls for Authority-backed refinement.
#![forbid(unsafe_code)]

use serde_json::{Value, json};
use zeno_fcis_contract::internals::review::evaluate::{digest, hex};
use zeno_fcis_contract::refinement::{self, Sources, Verdict};

struct Fixture {
    text: [String; 5],
}

impl Fixture {
    fn new() -> Self {
        Self {
            text: [
                include_str!(
                    "../../../examples/contract-refinement/reservation/abstract/project.zeno"
                )
                .to_owned(),
                include_str!(
                    "../../../examples/contract-refinement/reservation/abstract/v2/policy.json"
                )
                .to_owned(),
                include_str!(
                    "../../../examples/contract-refinement/reservation/concrete/project.zeno"
                )
                .to_owned(),
                include_str!(
                    "../../../examples/contract-refinement/reservation/concrete/v2/policy.json"
                )
                .to_owned(),
                include_str!("../../../examples/contract-refinement/reservation/abstraction.json")
                    .to_owned(),
            ],
        }
    }

    fn source(&self) -> Sources<'_> {
        Sources {
            abstract_project: &self.text[0],
            abstract_policy: &self.text[1],
            concrete_project: &self.text[2],
            concrete_policy: &self.text[3],
            abstraction: &self.text[4],
        }
    }

    fn edit(&mut self, slot: usize, f: impl FnOnce(&mut Value)) {
        let mut value: Value =
            serde_json::from_str(&self.text[slot]).unwrap_or_else(|e| panic!("{e}"));
        f(&mut value);
        self.text[slot] = value.to_string();
    }

    fn repin(&mut self) {
        let hashes: Vec<_> = self.text[..4]
            .iter()
            .map(|text| hex(&digest(text.as_bytes())))
            .collect();
        self.edit(4, |value| {
            for (name, hash) in [
                "abstract_project_sha256",
                "abstract_policy_sha256",
                "concrete_project_sha256",
                "concrete_policy_sha256",
            ]
            .iter()
            .zip(hashes)
            {
                value[*name] = json!(hash);
            }
        });
    }

    fn report(&self, work: u64) -> Value {
        refinement::check(self.source(), work)
            .unwrap_or_else(|e| panic!("{e}"))
            .json()
    }
}

fn row(policy: &mut Value, state: usize, input: usize) -> &mut Value {
    policy["cases"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("cases"))
        .iter_mut()
        .find(|case| case["when"] == format!("state == {state} && action == {input}"))
        .unwrap_or_else(|| panic!("row {state}/{input}"))
}

fn witness(value: &Value, obligation: &str, state: usize, input: usize) {
    assert_eq!(value["verdict"], "counterexample", "{value}");
    assert_eq!(
        value["witness"],
        json!({"obligation": obligation, "state": state, "input": input})
    );
    assert_eq!(value["checked_rows"], (state * 6 + input + 1) as u64);
}

#[test]
fn reservation_exhausts_384_raw_pairs_and_owns_its_report() {
    let fixture = Fixture::new();
    let before = fixture.text.clone();
    let report = refinement::check(fixture.source(), 384).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(report.verdict(), Verdict::Refines);
    let original = report.json();
    assert_eq!(original["raw_states"], 64);
    assert_eq!(original["abstract_states"], 26);
    assert_eq!(original["valid_states"], 26);
    assert_eq!(original["inputs"], 6);
    assert_eq!(original["checked_rows"], 384);
    assert_eq!(original["authority_evaluations"], 542); // both genesis + 384 concrete + 156 abstract
    assert_eq!(original["authority"], "none");
    assert_eq!(fixture.text, before);
    let mut independent = report.json();
    independent["verdict"] = json!("forged");
    assert_eq!(report.json(), original);
    assert_eq!(fixture.report(384), original);
}

#[test]
fn work_boundary_never_reports_refines_early() {
    let fixture = Fixture::new();
    for budget in [0, 1, 383] {
        let result = fixture.report(budget);
        assert_eq!(result["verdict"], "inconclusive");
        assert_eq!(result["checked_rows"], budget);
    }
    let zero = fixture.report(0);
    assert_eq!(zero["authority_evaluations"], 2);
    assert!(
        zero["authority_usage"][0]
            .as_str()
            .is_some_and(|n| n != "0")
    );
    assert_eq!(fixture.report(384)["verdict"], "refines");
}

#[test]
fn lost_retirement_fails_at_state_nine_input_three() {
    let mut fixture = Fixture::new();
    fixture.edit(3, |policy| row(policy, 9, 3)["post"]["110"] = json!("0"));
    fixture.repin();
    witness(&fixture.report(384), "successor-refinement", 9, 3);
    assert_eq!(fixture.report(57)["verdict"], "inconclusive");
    witness(&fixture.report(58), "successor-refinement", 9, 3);
}

#[test]
fn each_delivery_observation_part_is_compared() {
    for part in ["payload", "destination", "idempotency", "ordinal", "count"] {
        let mut fixture = Fixture::new();
        fixture.edit(3, |policy| {
            let event = &mut row(policy, 0, 0)["outbox"][0];
            match part {
                "payload" => event["payload"]["140"] = json!("1"),
                "destination" => event["destination"] = json!("elsewhere"),
                "idempotency" => event["idempotency_ordinal"] = json!(1),
                "ordinal" => event["ordinal"] = json!(1),
                "count" => row(policy, 0, 0)["outbox"] = json!([]),
                _ => unreachable!(),
            }
        });
        fixture.repin();
        let report = refinement::check(fixture.source(), 384)
            .unwrap_or_else(|e| panic!("{part}: {e}"))
            .json();
        witness(&report, "observation-refinement", 0, 0);
    }
}

#[test]
fn reasons_classes_and_delivery_channels_are_observed() {
    let mut fixture = Fixture::new();
    fixture.edit(3, |policy| row(policy, 9, 0)["reason"] = json!(201));
    fixture.repin();
    witness(&fixture.report(384), "observation-refinement", 9, 0);
    let mut fixture = Fixture::new();
    fixture.text[2] =
        fixture.text[2].replace("reason 204", "reason 205 failed precedence 5;\nreason 204");
    fixture.text[2].push_str("law 502 failure_allowed on failure = true;\n");
    fixture.edit(3, |policy| {
        policy["law_kinds"]["502"] = json!("CommittedFailureEffects");
        row(policy, 0, 0)["class"] = json!("CommittedFailure");
        row(policy, 0, 0)["reason"] = json!(205);
    });
    fixture.repin();
    witness(&fixture.report(384), "observation-refinement", 0, 0);
    let mut fixture = Fixture::new();
    fixture.text[2] = fixture.text[2]
        .replace("channel 300", "channel 301")
        .replace("outbox.300", "outbox.301");
    fixture.edit(3, |policy| {
        for case in policy["cases"]
            .as_array_mut()
            .unwrap_or_else(|| panic!("cases"))
        {
            for delivery in case["outbox"]
                .as_array_mut()
                .unwrap_or_else(|| panic!("outbox"))
            {
                delivery["channel"] = json!(301);
            }
        }
    });
    fixture.repin();
    witness(&fixture.report(384), "observation-refinement", 0, 0);
}

#[test]
fn initial_coverage_and_valid_to_invalid_are_checked() {
    let mut fixture = Fixture::new();
    fixture.edit(3, |policy| policy["genesis"]["110"] = json!(9));
    fixture.repin();
    assert_eq!(
        fixture.report(384)["witness"]["obligation"],
        "initial-state"
    );
    let mut fixture = Fixture::new();
    fixture.edit(4, |mapping| {
        for value in mapping["abstraction"]
            .as_array_mut()
            .unwrap_or_else(|| panic!("mapping"))
        {
            if *value == json!(1) {
                *value = json!(0);
            }
        }
    });
    assert_eq!(
        fixture.report(384)["witness"]["obligation"],
        "abstract-state-coverage"
    );
    let mut fixture = Fixture::new();
    fixture.edit(3, |policy| row(policy, 0, 0)["post"]["110"] = json!("1"));
    fixture.repin();
    witness(&fixture.report(384), "successor-refinement", 0, 0);
}

#[test]
fn invalid_states_need_the_exact_violated_law_and_state_major_order() {
    let mut fixture = Fixture::new();
    let at = fixture.text[2]
        .find("law 500")
        .unwrap_or_else(|| panic!("law"));
    let end = at
        + fixture.text[2][at..]
            .find(';')
            .unwrap_or_else(|| panic!("law end"))
        + 1;
    fixture.text[2].replace_range(at..end, "law 500 valid_representation on any = true;");
    fixture.repin();
    witness(&fixture.report(384), "malformed-state", 1, 0);
    let mut fixture = Fixture::new();
    fixture.edit(3, |policy| {
        row(policy, 9, 3)["post"]["110"] = json!("0");
        row(policy, 0, 1)["outbox"][0]["payload"]["140"] = json!("0");
    });
    fixture.repin();
    witness(&fixture.report(384), "observation-refinement", 0, 1);
}

#[test]
fn mapping_and_pins_fail_closed() {
    for mutation in [
        "pin", "unknown", "bool", "float", "length", "ordinal", "law",
    ] {
        let mut fixture = Fixture::new();
        fixture.edit(4, |value| match mutation {
            "pin" => value["abstract_project_sha256"] = json!("0".repeat(64)),
            "unknown" => value["trusted"] = json!(true),
            "bool" => value["abstraction"][0] = json!(true),
            "float" => value["abstraction"][0] = json!(0.0),
            "length" => value["abstraction"] = json!([0]),
            "ordinal" => value["abstraction"][0] = json!(26),
            "law" => value["invalid_state_law"] = json!(99999),
            _ => unreachable!(),
        });
        assert!(
            refinement::check(fixture.source(), 384).is_err(),
            "{mutation}"
        );
    }
    let mut fixture = Fixture::new();
    fixture.text[4] = fixture.text[4].replacen('{', "{\"invalid_state_law\":500,", 1);
    assert!(refinement::check(fixture.source(), 384).is_err());
    let fixture = Fixture::new();
    assert!(refinement::check(fixture.source(), refinement::MAX_CHECKS + 1).is_err());
}

#[test]
fn wide_domains_are_inconclusive_before_any_authority_work() {
    let mut fixture = Fixture::new();
    fixture.text[2] = fixture.text[2].replace("0..=63", "0..=9223372036854775807");
    fixture.repin();
    let report = fixture.report(384);
    assert_eq!(report["verdict"], "inconclusive");
    assert_eq!(report["reason"], "domain-over-limit");
    assert_eq!(report["authority_evaluations"], 0);
}

#[test]
fn a_reject_with_an_explicit_successor_is_refused_at_intake() {
    let mut fixture = Fixture::new();
    fixture.edit(3, |policy| row(policy, 9, 0)["post"]["110"] = json!("0"));
    fixture.repin();
    assert!(refinement::check(fixture.source(), 384).is_err());
}

#[test]
fn ordered_deliveries_and_genuine_genesis_are_checked() {
    let mut fixture = Fixture::new();
    for slot in [1, 3] {
        fixture.edit(slot, |policy| {
            let deliveries = row(policy, 0, 0)["outbox"]
                .as_array_mut()
                .unwrap_or_else(|| panic!("outbox"));
            let mut second = deliveries[0].clone();
            second["ordinal"] = json!(1);
            second["idempotency_ordinal"] = json!(1);
            second["payload"]["140"] = json!("1");
            deliveries.push(second);
        });
    }
    fixture.repin();
    assert_eq!(fixture.report(384)["verdict"], "refines");
    fixture.edit(3, |policy| {
        row(policy, 0, 0)["outbox"][0]["payload"]["140"] = json!("1");
        row(policy, 0, 0)["outbox"][1]["payload"]["140"] = json!("0");
    });
    fixture.repin();
    witness(&fixture.report(384), "observation-refinement", 0, 0);

    let mut fixture = Fixture::new();
    fixture.text[2] = fixture.text[2].replace("post.100.110 >= 0", "post.100.110 > 0");
    fixture.repin();
    let report = fixture.report(384);
    assert_eq!(report["verdict"], "counterexample");
    assert_eq!(report["witness"]["obligation"], "initial-admission");
    assert_eq!(report["checked_rows"], 0);
    assert_eq!(report["authority_evaluations"], 2);
}

#[test]
fn different_input_layout_and_oversized_sources_are_refused() {
    let mut fixture = Fixture::new();
    fixture.text[2] = fixture.text[2].replace("0..=5", "0..=6");
    fixture.repin();
    assert!(refinement::check(fixture.source(), 384).is_err());
    fixture.text[4] = " ".repeat(refinement::MAX_SOURCE_BYTES + 1);
    assert!(refinement::check(fixture.source(), 384).is_err());
}

#[test]
fn multiple_state_fields_allow_duplicate_representatives_without_aliasing() {
    let project = "zeno 1;\nproject 1 toggle;\nnamespace 2 example;\ntype 100 state State;\ntype 101 command Command;\ntype 102 context Context;\ntype 105 int Bit in 0..=1;\ntype 106 int Action in 0..=1;\ntype 107 int Unit in 0..=0;\nfield 110 100 value 105;\nfield 120 101 action 106;\nfield 130 102 unit 107;\nreason 200 denied precedence 0;\ncomponent 400 toggle { owns 100; reads pre.100; writes post.100; contexts context.102; budget steps 64; }\nmerge [400];\nlaw 501 bounded on commit, genesis = post.100.110 >= 0 && post.100.110 <= 1;\n";
    let policy = json!({
        "schema": "zeno-fcis/template-declarative-policy/2", "template": "toggle",
        "roots": {"state": 100, "command": 101, "context": 102}, "leaf_bindings": {}, "variables": {},
        "cases": [
            {"when": "command.101.120 == 0", "class": "Accept", "reason": null,
             "post": {"110": "1 - pre.100.110"}, "outbox": []},
            {"when": "true", "class": "Reject", "reason": 200, "post": {}, "outbox": []}
        ],
        "genesis": {"110": 0}, "law_kinds": {"501": "StateInvariant"}
    });
    let mut fixture = Fixture::new();
    fixture.text[0] = project.to_owned();
    fixture.text[1] = policy.to_string();
    fixture.text[2] = project.replace(
        "field 110",
        "type 108 bool Padding;\nfield 111 100 padding 108;\nfield 110",
    );
    fixture.text[3] = policy.to_string();
    fixture.edit(3, |value| {
        value["leaf_bindings"]["108"] = json!(["Bool"]);
        value["genesis"]["111"] = json!(false);
        value["cases"][0]["post"]["111"] = json!("pre.100.111");
    });
    fixture.edit(4, |value| value["abstraction"] = json!([0, 0, 1, 1]));
    fixture.repin();
    let result = fixture.report(8);
    assert_eq!(result["verdict"], "refines");
    assert_eq!(result["raw_states"], 4);
    assert_eq!(result["abstract_states"], 2);
    assert_eq!(result["valid_states"], 4);
    assert_eq!(result["authority_evaluations"], 18);
}

#[test]
fn wide_signed_inputs_preserve_native_refinement_without_narrowing() {
    let mut fixture = Fixture::new();
    for slot in [0, 2] {
        fixture.text[slot] = fixture.text[slot]
            .replace(
                "Action in 0..=5",
                "Action in 9223372036854775808..=9223372036854775813",
            )
            .replace(
                "Unit in 0..=0",
                "Unit in -9223372036854775809..=-9223372036854775809",
            )
            .replace(
                "field 140 104 event 106;",
                "type 108 int EventCode in 0..=5;\nfield 140 104 event 108;",
            );
    }
    for slot in [1, 3] {
        fixture.edit(slot, |policy| {
            policy["variables"]["action"] = json!("command.101.120 - 9223372036854775808");
        });
    }
    fixture.repin();
    let report = fixture.report(384);
    assert_eq!(report["verdict"], "refines", "{report}");
    assert_eq!(report["checked_rows"], 384);
    assert_eq!(report["authority_evaluations"], 542);
}
