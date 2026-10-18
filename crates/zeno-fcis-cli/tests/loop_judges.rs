//! `zeno-fcis loop question`, `loop teach-back` and `loop decompose` through
//! the real binary, on core-component contracts instantiated from their
//! templates. The cases translate the documented behaviour of the loops
//! these doors port: the same subjects, rules and architectures, and the
//! same verdicts, regions and suggested reads.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "support/arithmetic_judges.rs"]
mod arithmetic_fixture;

static NEXT: AtomicU64 = AtomicU64::new(0);
const CLI: &str = env!("CARGO_BIN_EXE_zeno-fcis");

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zeno-cli-loop-judges-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap_or_else(|error| panic!("create test directory: {error}"));
        Self(path)
    }

    fn json(&self, name: &str, value: &Value) -> String {
        let path = self.0.join(name);
        fs::write(&path, value.to_string()).unwrap_or_else(|error| panic!("write {name}: {error}"));
        path.to_string_lossy().into_owned()
    }

    /// A core component instantiated from its templates.
    fn contract(&self, family: &str, parameters: &[(&str, i128)]) -> String {
        let base = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../core-components")
            .join(family);
        let label: String = parameters
            .iter()
            .map(|(name, value)| format!("{}{value}", name.to_lowercase()))
            .collect::<Vec<_>>()
            .join("-");
        let app = self.0.join(format!("{family}-{label}"));
        fs::create_dir_all(app.join("v2")).unwrap_or_else(|error| panic!("{error}"));
        for (from, to) in [
            ("project.zeno.in", "project.zeno"),
            ("policy.json.in", "v2/policy.json"),
        ] {
            let mut text =
                fs::read_to_string(base.join(from)).unwrap_or_else(|error| panic!("{error}"));
            for (name, value) in parameters {
                text = text.replace(&format!("${{{name}}}"), &value.to_string());
            }
            text = text.replace("${INSTANCE}", &label);
            fs::write(app.join(to), text).unwrap_or_else(|error| panic!("{error}"));
        }
        app.to_string_lossy().into_owned()
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(arguments: &[&str]) -> (Option<i32>, Value) {
    let output = Command::new(CLI)
        .args(arguments)
        .output()
        .unwrap_or_else(|error| panic!("run zeno-fcis: {error}"));
    assert!(output.stderr.is_empty(), "{output:?}");
    let report = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("stdout is one JSON document: {error}: {output:?}"));
    (output.status.code(), report)
}

fn answers(items: &[Value]) -> Value {
    json!({"schema": "zeno-fcis/loop-answers/1", "answers": items})
}

fn arithmetic_contract(directory: &Directory, successor: &str, condition: &str) -> String {
    let app = directory.0.join("arithmetic");
    fs::create_dir_all(app.join("v2")).unwrap();
    fs::write(app.join("project.zeno"), arithmetic_fixture::project()).unwrap();
    fs::write(
        app.join("v2/policy.json"),
        arithmetic_fixture::policy(successor, condition).to_string(),
    )
    .unwrap();
    app.to_string_lossy().into_owned()
}

fn arithmetic_rule(id: &str, condition: &str, successor: &str) -> Value {
    json!({"id":id,"where":condition,"class":"Accept","reason":null,"sets":{"value":successor}})
}

#[test]
fn arithmetic_teach_back_cli_rejects_wrong_rounding_and_undefined_successors() {
    for (target, wrong) in [
        (arithmetic_fixture::FLOOR, arithmetic_fixture::CEIL),
        (arithmetic_fixture::CEIL, arithmetic_fixture::FLOOR),
    ] {
        let directory = Directory::new();
        let contract = arithmetic_contract(&directory, target, "true");
        let rules = directory.json(
            "rules.json",
            &json!({"schema":"zeno-fcis/loop-rules/1", "rules":[
                arithmetic_rule("good", "true", target),
                arithmetic_rule("wrong", "true", wrong),
                arithmetic_rule("floor-zero", "true", "mul_div_floor(amount, 1, 0)"),
                arithmetic_rule("ceil-zero", "true", "mul_div_ceil(amount, 1, 0)"),
            ]}),
        );
        let (exit, report) = run(&[
            "loop",
            "teach-back",
            "--contract",
            &contract,
            "--rule",
            &rules,
        ]);
        assert_eq!(exit, Some(0), "{report}");
        assert_eq!(report["authority"], "none");
        let judged = &report["detail"]["rules"];
        assert_eq!(judged[0]["verdict"], "exact", "{report}");
        assert_eq!(judged[1]["verdict"], "wrong", "{report}");
        assert_eq!(judged[1]["exceptions"], 4);
        assert_eq!(judged[2]["verdict"], "refused", "{report}");
        assert_eq!(judged[3]["verdict"], "refused", "{report}");
    }
}

#[test]
fn arithmetic_teach_back_cli_checks_both_product_conditions() {
    for (good, bad) in [
        (
            "products_ge(amount, 2, 3, 1)",
            "products_ge(amount, 3, 3, 1)",
        ),
        (
            "products_ge(amount, 2, 1, 3, 1, 1)",
            "products_ge(amount, 3, 1, 3, 1, 1)",
        ),
    ] {
        let directory = Directory::new();
        let contract = arithmetic_contract(&directory, arithmetic_fixture::FLOOR, good);
        let rules = directory.json("rules.json", &json!({"schema":"zeno-fcis/loop-rules/1", "rules":[
            arithmetic_rule("good", good, arithmetic_fixture::FLOOR),
            arithmetic_rule("wrong", bad, arithmetic_fixture::FLOOR),
            arithmetic_rule("floor-zero", "products_ge(mul_div_floor(amount, 1, 0), 1, 0, 1)", arithmetic_fixture::FLOOR),
            arithmetic_rule("ceil-zero", "products_ge(mul_div_ceil(amount, 1, 0), 1, 1, 0, 1, 1)", arithmetic_fixture::FLOOR),
        ]}));
        let (exit, report) = run(&[
            "loop",
            "teach-back",
            "--contract",
            &contract,
            "--rule",
            &rules,
        ]);
        assert_eq!(exit, Some(0), "{report}");
        let judged = &report["detail"]["rules"];
        assert_eq!(judged[0]["verdict"], "exact", "{report}");
        assert_eq!(judged[1]["verdict"], "wrong", "{report}");
        assert_eq!(judged[1]["exceptions"], 4);
        assert_eq!(judged[2]["verdict"], "refused", "{report}");
        assert_eq!(judged[3]["verdict"], "refused", "{report}");
    }
}

#[test]
fn the_largest_open_region_becomes_an_exact_question_and_answers_pin_it() {
    let directory = Directory::new();
    let c8 = directory.contract("bounded-counter", &[("C", 8)]);
    let (exit, report) = run(&["loop", "question", "--contract", &c8]);
    assert_eq!(exit, Some(0), "{report}");
    assert_eq!(report["schema"], "zeno-fcis/loop-doors/2");
    assert_eq!(report["authority"], "none");
    let detail = &report["detail"];
    // the declared laws determine no decision of the counter: five regions
    assert_eq!(detail["stats"]["determined"], 0);
    assert_eq!(detail["stats"]["regions"], 5);
    let question = &detail["question"];
    assert_eq!(question["case"], 0);
    assert_eq!(question["tuples"], 18);
    assert_eq!(question["where"], "!authorized");
    assert_eq!(question["where_law"], "!(context.102.130 == 1)");
    let options: Vec<(Value, Value)> = question["options"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .map(|o| (o["reason"].clone(), o["current"].clone()))
        .collect();
    assert_eq!(
        options,
        vec![
            (json!("not_authorized"), json!(true)),
            (json!("at_capacity"), json!(false)),
            (json!("at_zero"), json!(false)),
        ]
    );
    // the core's own answer: recorded, half the decisions now determined,
    // checked by the gaps judge with no false alarm
    let agree =
        json!({"where": "!authorized", "class": "Reject", "reason": "not_authorized", "case": 0});
    let path = directory.json("agree.json", &answers(std::slice::from_ref(&agree)));
    let (exit, report) = run(&["loop", "question", "--contract", &c8, "--answers", &path]);
    assert_eq!(exit, Some(0), "{report}");
    let detail = &report["detail"];
    assert_eq!(detail["answers"][0]["status"], "recorded");
    assert_eq!(detail["answers"][0]["reason_requirement"], "not_authorized");
    assert_eq!(detail["stats"]["determined"], 18);
    assert_eq!(detail["gaps"]["status"], "measured");
    // the next question never names an input already determined
    assert!(
        !detail["question"]["where"]
            .as_str()
            .unwrap_or_default()
            .is_empty()
    );
    assert_ne!(detail["question"]["case"], 0);
    // another reason: a disagreement, reported and never adopted
    let other = json!({"where": "!authorized", "class": "Reject", "reason": "at_zero", "case": 0});
    let path = directory.json("other.json", &answers(&[other]));
    let (exit, report) = run(&["loop", "question", "--contract", &c8, "--answers", &path]);
    assert_eq!(exit, Some(0), "{report}");
    let answer = &report["detail"]["answers"][0];
    assert_eq!(answer["status"], "disagreement");
    assert_eq!(answer["disagreement"]["inputs"], 18);
    assert_eq!(answer["disagreement"]["core"]["reason"], "not_authorized");
    // a different class: the gaps judge finds the pinned laws' false alarm
    let reject = json!({"where": "authorized && action == Inc && value == 3", "class": "Reject", "reason": "at_capacity"});
    let path = directory.json("reject.json", &answers(std::slice::from_ref(&reject)));
    let (exit, report) = run(&["loop", "question", "--contract", &c8, "--answers", &path]);
    assert_eq!(exit, Some(0), "{report}");
    let answer = &report["detail"]["answers"][0];
    assert_eq!(answer["status"], "disagreement");
    assert!(answer["false_alarm"]["law"].is_u64(), "{report}");
    assert_eq!(answer["false_alarm"]["law_origin"]["answer"], 0);
    assert_eq!(answer["false_alarm"]["law_origin"]["scope"], "accept");
    assert!(
        answer["false_alarm"]["law_origin"]["formula"]
            .as_str()
            .is_some_and(|f| f.contains("pre.100.110"))
    );
    assert_eq!(report["detail"]["gaps"]["status"], "proposal-false-alarm");
    // an answer that contradicts an earlier one: refused, nothing recorded
    let accept =
        json!({"where": "authorized && action == Inc && value == 3", "class": "Accept", "case": 3});
    let path = directory.json("conflict.json", &answers(&[reject, accept]));
    let (exit, report) = run(&["loop", "question", "--contract", &c8, "--answers", &path]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["status"], "conflict");
    assert_eq!(report["detail"]["answer"], 1);
    assert_eq!(report["detail"]["example"]["value"], 3);
    assert_eq!(report["detail"]["example"]["action"], "Inc");
    assert_eq!(report["detail"]["requirement"]["class"], "Accept");
    assert_eq!(report["detail"]["earlier_answers"][0]["answer"], 0);
}

#[test]
fn rules_are_judged_exactly_with_regions_and_a_summary() {
    let directory = Directory::new();
    let c3 = directory.contract("bounded-counter", &[("C", 3)]);
    let rule = |id: &str, where_: &str, class: &str, reason: Value, sets: Value| {
        let mut out = json!({"id": id, "where": where_, "class": class, "reason": reason});
        if !sets.is_null() {
            out["sets"] = sets;
        }
        out
    };
    let rules = json!({"schema": "zeno-fcis/loop-rules/1", "rules": [
        rule("t2", "authorized && action == Inc && value == 3", "Reject", json!("at_capacity"), Value::Null),
        rule("t1", "action == Inc && value == 3", "Reject", json!("at_capacity"), Value::Null),
        rule("t3", "authorized && action == Inc && value < 2", "Accept", Value::Null, json!({"value": "value + 1"})),
        rule("o", "!authorized && (170141183460469231731687303715884105727 + 1 < 0)", "Reject", json!("not_authorized"), Value::Null),
    ]});
    let path = directory.json("rules.json", &rules);
    let (exit, report) = run(&["loop", "teach-back", "--contract", &c3, "--rule", &path]);
    assert_eq!(exit, Some(0), "{report}");
    let judged = &report["detail"]["rules"];
    assert_eq!(judged[0]["verdict"], "exact");
    assert_eq!(judged[1]["verdict"], "wrong");
    assert_eq!(judged[1]["exceptions"], 1);
    let group = &judged[1]["exception_groups"][0];
    assert_eq!(group["actual"]["reason"], "not_authorized");
    assert_eq!(group["input"]["authorized"], false);
    assert_eq!(judged[2]["verdict"], "holds");
    assert_eq!(judged[2]["missing"], 1);
    assert_eq!(judged[2]["missing_region"]["input"]["value"], 2);
    // an overflowing condition is refused, never judged
    assert_eq!(judged[3]["verdict"], "refused");
    // five exact rules cover the whole domain without overlap
    let summary = json!({"schema": "zeno-fcis/loop-rules/1", "rules": [
        rule("a", "!authorized", "Reject", json!("not_authorized"), Value::Null),
        rule("b", "authorized && action == Inc && value == 3", "Reject", json!("at_capacity"), Value::Null),
        rule("c", "authorized && action == Inc && value < 3", "Accept", Value::Null, json!({"value": "value + 1"})),
        rule("d", "authorized && action == Dec && value > 0", "Accept", Value::Null, json!({"value": "value - 1"})),
        rule("e", "authorized && action == Dec && value == 0", "Reject", json!("at_zero"), Value::Null),
    ]});
    let path = directory.json("summary.json", &summary);
    let (exit, report) = run(&["loop", "teach-back", "--contract", &c3, "--rule", &path]);
    assert_eq!(exit, Some(0), "{report}");
    let summary = &report["detail"]["summary"];
    assert_eq!(
        (
            summary["exact"].clone(),
            summary["complete"].clone(),
            summary["overlapping"].clone()
        ),
        (json!(5), json!(true), json!(0))
    );
    // the same rule in other words has the same meaning; with or without a
    // successor, or with another reason, it does not
    let keys = json!({"schema": "zeno-fcis/loop-rules/1", "rules": [
        rule("k1", "authorized && action == Inc && value < 3", "Accept", Value::Null, json!({"value": "value + 1"})),
        rule("k2", "action == Inc && authorized && !(value >= 3)", "Accept", Value::Null, json!({"value": "1 + value"})),
        rule("k3", "authorized && action == Inc && value < 3", "Accept", Value::Null, Value::Null),
        rule("k4", "!authorized", "Reject", json!("not_authorized"), Value::Null),
        rule("k5", "!authorized", "Reject", json!("at_zero"), Value::Null),
    ]});
    let path = directory.json("keys.json", &keys);
    let (_, report) = run(&["loop", "teach-back", "--contract", &c3, "--rule", &path]);
    let meaning = |k: usize| report["detail"]["rules"][k]["meaning"].clone();
    assert_eq!(meaning(0), meaning(1));
    assert_ne!(meaning(0), meaning(2));
    assert_ne!(meaning(3), meaning(4));
}

fn architecture(components: &[Value]) -> Value {
    json!({"schema": "zeno-fcis/loop-architecture/1", "components": components})
}

#[test]
fn a_decomposition_is_projected_explained_or_blamed() {
    let directory = Directory::new();
    let c3 = directory.contract("bounded-counter", &[("C", 3)]);
    let guard = json!({"name": "guard", "reads": ["authorized"], "decides": ["not_authorized"]});
    // a skeleton: both case tables are projected from the core
    let path = directory.json(
        "skeleton.json",
        &architecture(&[
            guard.clone(),
            json!({"name": "counter", "owns": ["value"], "reads": ["action"], "decides": ["at_capacity", "at_zero"]}),
        ]),
    );
    let (exit, report) = run(&[
        "loop",
        "decompose",
        "--contract",
        &c3,
        "--architecture",
        &path,
    ]);
    assert_eq!(exit, Some(0), "{report}");
    let detail = &report["detail"];
    assert_eq!(report["status"], "completed");
    assert_eq!(detail["local_domain_total"], "10");
    let table = &detail["projection"]["guard"]["table"];
    assert_eq!(table[0]["class"], "Reject");
    assert_eq!(table[1]["class"], "Accept");
    assert_eq!(table[1]["when"], "true");
    // a counter that does not read the action: an interface counterexample
    // naming the missing read
    let path = directory.json(
        "blind.json",
        &architecture(&[
            guard.clone(),
            json!({"name": "counter", "owns": ["value"], "decides": ["at_capacity", "at_zero"]}),
        ]),
    );
    let (exit, report) = run(&[
        "loop",
        "decompose",
        "--contract",
        &c3,
        "--architecture",
        &path,
    ]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["status"], "unrealizable");
    let clash = &report["detail"]["interface"][0];
    assert_eq!(clash["component"], "counter");
    assert_eq!(clash["suggested_reads"], json!(["action"]));
    assert_eq!(clash["inputs"][0]["value"], clash["inputs"][1]["value"]);
    // a counter that steps below zero: blamed with a region, the door marking
    // the out-of-domain successor refused
    let path = directory.json(
        "wrong.json",
        &architecture(&[
            json!({"name": "guard", "reads": ["authorized"], "decides": ["not_authorized"],
                   "cases": [{"when": "!authorized", "class": "Reject", "reason": "not_authorized"},
                             {"when": "true", "class": "Accept"}]}),
            json!({"name": "counter", "owns": ["value"], "reads": ["action"], "decides": ["at_capacity", "at_zero"],
                   "cases": [{"when": "action == Inc && value == 3", "class": "Reject", "reason": "at_capacity"},
                             {"when": "action == Inc", "class": "Accept", "sets": {"value": "value + 1"}},
                             {"when": "true", "class": "Accept", "sets": {"value": "value - 1"}}]}),
        ]),
    );
    let (exit, report) = run(&[
        "loop",
        "decompose",
        "--contract",
        &c3,
        "--architecture",
        &path,
    ]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["status"], "refuted");
    let blame = &report["detail"]["blame"][0];
    assert_eq!(blame["component"], "counter");
    assert_eq!(blame["wrong"], 1);
    assert_eq!(blame["required"]["reason"], "at_zero");
    assert_eq!(blame["actual"]["class"], "Refused");
    assert!(blame["where"].is_string(), "{blame}");
    // a reason no component decides is reported
    let path = directory.json(
        "unowned.json",
        &architecture(&[
            guard,
            json!({"name": "counter", "owns": ["value"], "reads": ["action"], "decides": ["at_capacity"]}),
        ]),
    );
    let (_, report) = run(&[
        "loop",
        "decompose",
        "--contract",
        &c3,
        "--architecture",
        &path,
    ]);
    let problems = &report["detail"]["structure"];
    assert_eq!(problems[0]["kind"], "unowned-reason");
    assert_eq!(problems[0]["reason"], "at_zero");
}

#[test]
fn reads_come_from_earlier_components_and_a_reorder_is_explained() {
    let directory = Directory::new();
    let k2 = directory.contract("approval-queue", &[("K", 2)]);
    let ballot = json!({"name": "ballot", "owns": ["vote0", "vote1", "vote2"],
                        "reads": ["action", "principal", "authorized", "phase"],
                        "decides": ["not_authorized", "duplicate_vote", "quorum_missing"]});
    let path = directory.json(
        "later.json",
        &architecture(&[
            json!({"name": "phase", "owns": ["phase"], "reads": ["action", "vote0"],
                   "decides": ["invalid_state", "wrong_phase"]}),
            ballot.clone(),
        ]),
    );
    let (exit, report) = run(&[
        "loop",
        "decompose",
        "--contract",
        &k2,
        "--architecture",
        &path,
    ]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["status"], "unrealizable");
    assert_eq!(report["detail"]["structure"][0]["what"], "reads-later");
    assert_eq!(report["detail"]["structure"][0]["field"], "vote0");
    assert_eq!(
        report["detail"]["structure"][0]["suggested_order"],
        Value::Null
    );
    assert_eq!(report["detail"]["structure"][0]["cyclic_reads"], true);
    let path = directory.json(
        "reorder.json",
        &architecture(&[
            json!({"name": "phase", "owns": ["phase"], "reads": ["action"],
                   "decides": ["invalid_state", "wrong_phase"]}),
            ballot,
        ]),
    );
    let (_, report) = run(&[
        "loop",
        "decompose",
        "--contract",
        &k2,
        "--architecture",
        &path,
    ]);
    let clash = &report["detail"]["interface"][0];
    assert_eq!(clash["component"], "phase");
    assert_eq!(
        clash["suggested_reads_owned_later"],
        json!(["vote0", "vote1", "vote2"])
    );
    let path = directory.json(
        "three.json",
        &architecture(&[
            json!({"name": "state", "owns": ["phase", "vote0", "vote1", "vote2"],
                   "reads": ["action", "principal"], "decides": ["invalid_state"]}),
            json!({"name": "guard", "reads": ["authorized"], "decides": ["not_authorized"]}),
            json!({"name": "rules", "reads": ["phase", "vote0", "vote1", "vote2", "action", "principal"],
                   "decides": ["wrong_phase", "duplicate_vote", "quorum_missing"]}),
        ]),
    );
    let (exit, report) = run(&[
        "loop",
        "decompose",
        "--contract",
        &k2,
        "--architecture",
        &path,
    ]);
    assert_eq!(exit, Some(0), "{report}");
    assert_eq!(report["status"], "completed");
}

#[test]
fn malformed_requests_are_refused() {
    let directory = Directory::new();
    let c3 = directory.contract("bounded-counter", &[("C", 3)]);
    for (name, document, command, flag) in [
        ("a.json", json!({"schema": "x"}), "question", "--answers"),
        (
            "r.json",
            json!({"schema": "zeno-fcis/loop-rules/1", "rules": [{"where": "nosuch", "class": "Accept"}]}),
            "teach-back",
            "--rule",
        ),
        (
            "d.json",
            json!({"schema": "zeno-fcis/loop-architecture/1", "components": [{"name": "x", "owns": ["nosuch"]}]}),
            "decompose",
            "--architecture",
        ),
    ] {
        let path = directory.json(name, &document);
        let (exit, report) = run(&["loop", command, "--contract", &c3, flag, &path]);
        assert_ne!(exit, Some(0), "{command}: {report}");
    }
}

#[test]
fn equal_rule_keys_give_equal_verdicts_and_the_successor_counts_outside_the_region() {
    let directory = Directory::new();
    let c3 = directory.contract("bounded-counter", &[("C", 3)]);
    // the same region and claim; the second rule's successor differs only
    // where the rule does not apply, which teach-back compares for the
    // inputs a rule misses
    let rules = json!({"schema": "zeno-fcis/loop-rules/1", "rules": [
        {"id": "a", "where": "authorized && action == Inc && value < 3", "class": "Accept",
         "sets": {"value": "value + 1"}},
        {"id": "b", "where": "authorized && action == Inc && value < 3", "class": "Accept",
         "sets": {"value": "action == Inc ? value + 1 : value - 1"}},
        {"id": "c", "where": "action == Inc && authorized && !(value >= 3)", "class": "Accept",
         "sets": {"value": "1 + value"}},
    ]});
    let path = directory.json("rules.json", &rules);
    let (exit, report) = run(&["loop", "teach-back", "--contract", &c3, "--rule", &path]);
    assert_eq!(exit, Some(0), "{report}");
    let judged = &report["detail"]["rules"];
    assert_eq!(judged[0]["verdict"], "exact", "{report}");
    assert_eq!(judged[1]["verdict"], "holds", "{report}");
    assert_eq!(judged[1]["missing"], 3);
    assert_ne!(judged[0]["meaning"], judged[1]["meaning"]);
    // equal keys: every judged count and the verdict agree
    assert_eq!(judged[0]["meaning"], judged[2]["meaning"]);
    for field in [
        "verdict",
        "applies",
        "holds_on",
        "exceptions",
        "missing",
        "applies_reachable",
        "exceptions_reachable",
        "missing_reachable",
    ] {
        assert_eq!(judged[0][field], judged[2][field], "{field}");
    }
}

#[test]
fn a_negated_number_is_pinned_with_its_loop_meaning_and_a_mixed_comparison_is_refused() {
    let directory = Directory::new();
    let c3 = directory.contract("bounded-counter", &[("C", 3)]);
    // the loop language reads `!value` as `value == 0`: four inputs
    let negated = json!({"where": "!value", "class": "Reject", "reason": "at_zero"});
    let path = directory.json("negated.json", &answers(&[negated]));
    let (exit, report) = run(&["loop", "question", "--contract", &c3, "--answers", &path]);
    assert_eq!(exit, Some(0), "{report}");
    let answer = &report["detail"]["answers"][0];
    assert_eq!(answer["inputs"], 4, "{report}");
    let laws: Vec<String> = answer["laws"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .map(|l| l["formula"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert!(!laws.is_empty(), "{report}");
    for law in &laws {
        assert!(law.contains("pre.100.110 == 0"), "{law}");
        assert!(!law.contains("pre.100.110 == 1"), "{law}");
    }
    // forms with no exact law text are refused: a truth value compared
    // with a number, and a conditional whose branches mix the two (the
    // doors read `value ? authorized : value` as `value` when value is 2)
    for (name, where_, reason) in [
        (
            "mixed.json",
            "value == authorized",
            "law-compares-a-truth-value-with-a-number",
        ),
        (
            "negated-conditional.json",
            "!(value ? authorized : value)",
            "law-conditional-mixes-a-truth-value-and-a-number",
        ),
        (
            "compared-conditional.json",
            "authorized == (authorized ? authorized : value)",
            "law-conditional-mixes-a-truth-value-and-a-number",
        ),
    ] {
        let answer = json!({"where": where_, "class": "Reject", "reason": "at_zero"});
        let path = directory.json(name, &answers(&[answer]));
        let (exit, report) = run(&["loop", "question", "--contract", &c3, "--answers", &path]);
        assert_eq!(exit, Some(1), "{where_}: {report}");
        assert_eq!(report["status"], "refused", "{where_}: {report}");
        assert!(
            report["detail"]["reason"]
                .as_str()
                .unwrap_or_default()
                .contains(reason),
            "{where_}: {report}"
        );
    }
}

#[test]
fn an_answer_whose_successor_differs_from_the_core_is_a_disagreement() {
    let directory = Directory::new();
    let c8 = directory.contract("bounded-counter", &[("C", 8)]);
    // Without the exact-movement law (502) the laws admit any in-range
    // successor, so an answer can name another one without a conflict.
    let project = Path::new(&c8).join("project.zeno");
    let text = fs::read_to_string(&project).unwrap_or_else(|error| panic!("{error}"));
    let kept: Vec<&str> = text
        .lines()
        .filter(|line| !line.starts_with("law 502 "))
        .collect();
    fs::write(&project, kept.join("\n") + "\n").unwrap_or_else(|error| panic!("{error}"));
    let policy = Path::new(&c8).join("v2/policy.json");
    let mut value: Value = serde_json::from_str(
        &fs::read_to_string(&policy).unwrap_or_else(|error| panic!("{error}")),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    if let Some(kinds) = value["law_kinds"].as_object_mut() {
        kinds.remove("502");
    }
    fs::write(&policy, value.to_string()).unwrap_or_else(|error| panic!("{error}"));
    let items = [
        json!({"where": "authorized && action == Inc && value == 3", "class": "Reject", "reason": "at_capacity"}),
        json!({"where": "authorized && action == Inc && value == 5", "class": "Accept", "sets": {"value": "0"}}),
    ];
    let path = directory.json("successor.json", &answers(&items));
    let (exit, report) = run(&["loop", "question", "--contract", &c8, "--answers", &path]);
    assert_eq!(exit, Some(0), "{report}");
    let answer = &report["detail"]["answers"][1];
    assert_eq!(answer["status"], "disagreement", "{report}");
    assert_eq!(answer["disagreement"]["inputs"], 1);
    assert_eq!(answer["disagreement"]["core"]["sets"]["value"], 6);
    assert_eq!(answer["disagreement"]["owner"]["sets"]["value"], 0);
}

#[test]
fn structure_problems_make_a_decomposition_unrealizable_even_with_tables() {
    let directory = Directory::new();
    let c3 = directory.contract("bounded-counter", &[("C", 3)]);
    // no component decides not_authorized, so on those inputs no component
    // is required to do anything and no blame can arise there
    let path = directory.json(
        "unowned.json",
        &architecture(&[json!({"name": "all", "owns": ["value"], "reads": ["action", "authorized"],
            "decides": ["at_capacity", "at_zero"],
            "cases": [{"when": "action == Inc && value == 3", "class": "Reject", "reason": "at_capacity"},
                      {"when": "action == Dec && value == 0", "class": "Reject", "reason": "at_zero"},
                      {"when": "action == Inc", "class": "Accept", "sets": {"value": "value + 1"}},
                      {"when": "true", "class": "Accept", "sets": {"value": "value - 1"}}]})]),
    );
    let (exit, report) = run(&[
        "loop",
        "decompose",
        "--contract",
        &c3,
        "--architecture",
        &path,
    ]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["status"], "unrealizable", "{report}");
    let detail = &report["detail"];
    assert_eq!(detail["realizable"], false);
    assert_eq!(detail["structure"][0]["kind"], "unowned-reason");
    // steps 1 to 3 are the door's own evaluation
    assert_eq!(detail["decided_by"], "door-simulation");
}

#[test]
fn a_rule_the_confirmed_answers_contradict_is_forbidden_never_required() {
    let directory = Directory::new();
    let c3 = directory.contract("bounded-counter", &[("C", 3)]);
    let rules = directory.json(
        "old-core.json",
        &json!({"schema": "zeno-fcis/loop-rules/1", "rules": [
            {"id": "old_core", "where": "!authorized", "class": "Reject", "reason": "not_authorized"}]}),
    );
    // the owner answered another reason: a disagreement, not a conflict
    let other = directory.json(
        "other.json",
        &answers(&[json!({"where": "!authorized", "class": "Reject", "reason": "at_capacity"})]),
    );
    let (exit, report) = run(&["loop", "question", "--contract", &c3, "--answers", &other]);
    assert_eq!(exit, Some(0), "{report}");
    assert_eq!(report["detail"]["answers"][0]["status"], "disagreement");
    let (exit, report) = run(&[
        "loop",
        "teach-back",
        "--contract",
        &c3,
        "--rule",
        &rules,
        "--answers",
        &other,
    ]);
    assert_eq!(exit, Some(0), "{report}");
    let judged = &report["detail"]["rules"][0];
    // still an exact description of the core, but the answers forbid it
    assert_eq!(judged["verdict"], "exact");
    let required = &judged["required"];
    assert_eq!(required["of"], 8, "{required}");
    assert_eq!(required["forbidden"], 8, "{required}");
    assert_eq!(required["forced"], 0, "{required}");
    assert_eq!(required["open"], 0, "{required}");
    assert_eq!(
        required["forbidden_example"]["admitted"][0]["reason"],
        "at_capacity"
    );
    // the core's own answer forces the same rule
    let same = directory.json(
        "same.json",
        &answers(&[json!({"where": "!authorized", "class": "Reject", "reason": "not_authorized"})]),
    );
    let (_, report) = run(&[
        "loop",
        "teach-back",
        "--contract",
        &c3,
        "--rule",
        &rules,
        "--answers",
        &same,
    ]);
    let required = &report["detail"]["rules"][0]["required"];
    assert_eq!(
        (
            required["forced"].clone(),
            required["forbidden"].clone(),
            required["open"].clone()
        ),
        (json!(8), json!(0), json!(0)),
        "{report}"
    );
}

#[test]
fn a_question_over_boolean_votes_writes_their_sum_as_the_laws_do() {
    let directory = Directory::new();
    // the approval queue's laws sum Boolean votes as 0 and 1; a question
    // region over them is written the same way, never refused
    let queue = directory.contract("approval-queue", &[("K", 1)]);
    let (exit, report) = run(&["loop", "question", "--contract", &queue]);
    assert_eq!(exit, Some(0), "{report}");
    assert_eq!(report["status"], "open", "{report}");
    assert!(
        report["detail"]["question"]["where_law"]
            .as_str()
            .unwrap_or_default()
            .contains(" + "),
        "{report}"
    );
}

#[test]
fn truthful_counter_answers_distinguish_laws_from_recorded_reasons() {
    let directory = Directory::new();
    let c3 = directory.contract("bounded-counter", &[("C", 3)]);
    let document: Value = serde_json::from_str(include_str!(
        "../../zeno-fcis-loop-doors/tests/data/bc-c3-explicit-answers.json"
    ))
    .unwrap_or_else(|error| panic!("fixture: {error:?}"));
    let path = directory.json("all-answers.json", &document);
    let (exit, report) = run(&["loop", "question", "--contract", &c3, "--answers", &path]);
    assert_eq!(exit, Some(0), "{report}");
    assert_eq!(report["detail"]["stats"]["determined_by_laws"], 6);
    assert_eq!(report["detail"]["stats"]["determined_with_reasons"], 16);
    assert_eq!(report["detail"]["stats"]["determined"], 16);
    assert_eq!(
        report["detail"]["gaps"]["determined"],
        json!({"full":[0,6],"valid":[0,6]})
    );
}

#[test]
fn later_owned_votes_are_unrealizable_in_this_order_with_a_reorder_suggestion() {
    let directory = Directory::new();
    let queue = directory.contract("approval-queue", &[("K", 1)]);
    let document: Value = serde_json::from_str(include_str!(
        "../../zeno-fcis-loop-doors/tests/data/aq-arch.json"
    ))
    .unwrap_or_else(|error| panic!("fixture: {error:?}"));
    let path = directory.json("later-votes.json", &document);
    let (exit, report) = run(&[
        "loop",
        "decompose",
        "--contract",
        &queue,
        "--architecture",
        &path,
    ]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["status"], "unrealizable");
    let problem = &report["detail"]["structure"][0];
    assert_eq!(problem["component"], "lifecycle");
    assert_eq!(problem["field"], "vote0");
    assert_eq!(problem["suggested_order"], json!(["ballot", "lifecycle"]));
    assert_eq!(problem["cyclic_reads"], false);
    assert!(report["detail"].get("compose").is_none());
}
