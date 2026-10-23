use super::*;
fn fixture(cases: Value, extra: &str, mut kinds: Value) -> (String, String) {
    kinds["599"] = json!("StateInvariant");
    let project = format!(
        "zeno 1; project 1 two_run_test; namespace 2 core;\n\
        type 100 state State; type 101 command Command; type 102 context Context;\n\
        type 105 int Amount in 0..=2; type 106 bool Flag; type 107 int Action in 0..=1;\n\
        field 110 100 marker 105; field 111 100 value 105; field 112 100 secret 106;\n\
        field 120 101 action 107;\n\
        component 400 test {{ owns 100; reads pre.100; writes post.100; contexts context.102; budget steps 256; }} merge [400];\nlaw 599 bounded on commit, genesis = post.100.111 >= 0;\n{extra}"
    );
    let rules = json!({"schema":"zeno-fcis/template-declarative-policy/2","template":"two-run-test","roots":{"state":100,"command":101,"context":102},"leaf_bindings":{"102":["Bool"],"106":["Bool"]},"variables":{},"cases":cases,"genesis":{"110":0,"111":0,"112":false},"law_kinds":kinds}).to_string();
    (project, rules)
}
fn accept(post: Value) -> Value {
    json!({"when":true,"class":"Accept","reason":null,"post":post,"outbox":[]})
}
fn order(observe: &[&str], within: u128) -> Property {
    Property::Commute {
        first: "command.101.120 == 0".into(),
        second: "command.101.120 == 1".into(),
        observe: observe.iter().map(|s| s.to_string()).collect(),
        within,
    }
}
fn private() -> Property {
    Property::Noninterference {
        secret: vec!["pre.100.112".into()],
        public: vec!["post.100.111".into()],
    }
}
fn checked(p: &str, r: &str, property: &Property) -> CheckResult {
    check(p, r, property, MAX_TUPLES).unwrap_or_else(|e| panic!("{e}"))
}
#[test]
fn compares_every_observed_field() {
    let (p, r) = fixture(
        json!([accept(json!({"111":"command.101.120"}))]),
        "",
        json!({}),
    );
    let result = checked(&p, &r, &order(&["post.100.110", "post.100.111"], 0));
    assert_eq!(result.verdict(), Verdict::Refuted);
    assert_eq!(
        result.packet()["result"]["first_difference"]["path"],
        "pre.100.111"
    );
    assert_eq!(
        checked(&p, &r, &order(&["pre.100.110", "pre.100.111"], 1)).verdict(),
        Verdict::Checked
    );
}
#[test]
fn refuses_in_only_one_order_is_a_difference() {
    let (p, r) = fixture(
        json!([accept(
            json!({"111":"choose(command.101.120 == 0, 2, pre.100.111 + 1)"})
        )]),
        "",
        json!({}),
    );
    let result = checked(&p, &r, &order(&["pre.100.110"], 0));
    assert_eq!(result.verdict(), Verdict::Refuted);
    assert_eq!(
        result.packet()["result"]["first_difference"]["kind"],
        "decision_class"
    );
    assert_eq!(
        result.packet()["result"]["first_difference"]["right"],
        "Accept"
    );
}
#[test]
fn secret_group_excludes_secret_and_detects_leak() {
    let (p, r) = fixture(
        json!([accept(json!({"111":"choose(pre.100.112, 1, 0)"}))]),
        "",
        json!({}),
    );
    let result = checked(&p, &r, &private());
    assert_eq!(result.verdict(), Verdict::Refuted);
    assert_eq!(
        result.packet()["result"]["first_difference"]["kind"],
        "public_field"
    );
}
#[test]
fn independent_secret_is_linear_and_deterministic() {
    let (p, r) = fixture(
        json!([accept(json!({"111":"command.101.120"}))]),
        "",
        json!({}),
    );
    let one = checked(&p, &r, &private());
    let two = checked(&p, &r, &private());
    assert_eq!(one.verdict(), Verdict::Checked);
    assert_eq!(one.packet(), two.packet());
    assert_eq!(one.packet()["measured"]["authority_invocations"], 72);
    assert_eq!(one.packet()["measured"]["comparisons"], 36);
}
#[test]
fn rejects_are_no_op_and_compared_by_input_identity() {
    let reject =
        json!({"when":"command.101.120 == 0","class":"Reject","reason":200,"post":{},"outbox":[]});
    let (p, r) = fixture(
        json!([reject, accept(json!({"111":1}))]),
        "reason 200 denied precedence 0;",
        json!({}),
    );
    let result = checked(&p, &r, &order(&["pre.100.111"], 0));
    assert_eq!(result.verdict(), Verdict::Checked);
    let (p, r) = fixture(
        json!([{"when":"pre.100.112","class":"Reject","reason":200,"post":{},"outbox":[]},{"when":true,"class":"Reject","reason":201,"post":{},"outbox":[]}]),
        "reason 200 a precedence 0; reason 201 b precedence 1;",
        json!({}),
    );
    let result = checked(&p, &r, &private());
    assert_eq!(result.verdict(), Verdict::Refuted);
    assert_eq!(
        result.packet()["result"]["first_difference"]["kind"],
        "reason"
    );
}
#[test]
fn state_laws_exclude_inadmissible_states() {
    let (p, r) = fixture(
        json!([accept(json!({}))]),
        "law 500 allowed on commit, genesis = post.100.112 == 0;",
        json!({"500":"StateInvariant"}),
    );
    let result = checked(&p, &r, &private());
    assert_eq!(result.verdict(), Verdict::Checked);
    assert_eq!(result.packet()["measured"]["admissible_states"], 9);
    assert_eq!(result.packet()["measured"]["comparisons"], 0);
}
#[test]
fn validation_and_incompleteness_never_pass() {
    let (p, r) = fixture(json!([accept(json!({}))]), "", json!({}));
    assert!(check(&p, &r, &order(&["pre.100.999"], 0), MAX_TUPLES).is_err());
    let both = Property::Noninterference {
        secret: vec!["pre.100.112".into()],
        public: vec!["post.100.112".into()],
    };
    assert!(check(&p, &r, &both, MAX_TUPLES).is_err());
    let mut prop = order(&["pre.100.111"], 0);
    if let Property::Commute { first, .. } = &mut prop {
        *first = "pre.100.111 == 0".into();
    }
    assert!(check(&p, &r, &prop, MAX_TUPLES).is_err());
    if let Property::Commute { first, .. } = &mut prop {
        *first = "div_floor(1, 0) == 0".into();
    }
    assert_eq!(checked(&p, &r, &prop).verdict(), Verdict::Unknown);
    if let Property::Commute { first, .. } = &mut prop {
        *first = "false".into();
    }
    assert_eq!(checked(&p, &r, &prop).verdict(), Verdict::Unknown);
    assert_eq!(
        check(&p, &r, &private(), 1)
            .unwrap_or_else(|e| panic!("{e}"))
            .verdict(),
        Verdict::Unknown
    );
}
#[test]
fn context_is_part_of_each_complete_input() {
    let (p, r) = fixture(
        json!([accept(json!({"111":"choose(context.102, 1, 0)"}))]),
        "",
        json!({}),
    );
    let prop = Property::Commute {
        first: "context.102".into(),
        second: "!context.102".into(),
        observe: vec!["pre.100.111".into()],
        within: 0,
    };
    assert_eq!(checked(&p, &r, &prop).verdict(), Verdict::Refuted);
}
#[test]
fn distance_handles_signed_endpoints_without_overflow() {
    let run = |n| Run {
        outcomes: [
            Outcome::Refused(evaluate::Refusal {
                text: "r".into(),
                class: evaluate::RefusalClass::Other,
                law: None,
            }),
            Outcome::Refused(evaluate::Refusal {
                text: "r".into(),
                class: evaluate::RefusalClass::Other,
                law: None,
            }),
        ],
        final_state: vec![n],
    };
    let p = Position {
        name: "pre.100.110".into(),
        source: Source::State,
        field: Some(110),
        type_id: 105,
        domain: LeafDomain::Int {
            min: i128::MIN,
            max: i128::MAX,
        },
    };
    assert!(
        difference(
            &run(i128::MIN),
            &run(i128::MAX),
            &[0],
            u128::MAX - 1,
            &[p.clone()]
        )
        .is_some()
    );
    assert!(difference(&run(i128::MIN), &run(i128::MAX), &[0], u128::MAX, &[p]).is_none());
}
#[test]
fn outbox_and_committed_failure_are_observed() {
    let mut c = accept(json!({}));
    c["outbox"] = json!([{"ordinal":0,"channel":300,"destination":"public","payload":{"130":"choose(pre.100.112, 1, 0)"},"idempotency_ordinal":0}]);
    let (mut p, r) = fixture(json!([c]), "", json!({}));
    p.push_str("\ntype 103 destination Desk; type 104 payload Notice; field 130 104 value 105; channel 300 notify destination 103 payload 104;");
    let mut policy: Value = serde_json::from_str(&r).unwrap_or_else(|e| panic!("{e}"));
    policy["leaf_bindings"]["103"] = json!(["Text", 1, 16]);
    let result = checked(&p, &policy.to_string(), &private());
    assert_eq!(result.verdict(), Verdict::Refuted);
    assert_eq!(
        result.packet()["result"]["first_difference"]["kind"],
        "outbox"
    );
    let (p, r) = fixture(
        json!([{"when":true,"class":"CommittedFailure","reason":201,"post":{"111":"command.101.120"},"outbox":[]}]),
        "reason 201 failed precedence 0; law 501 failure_effect on failure = post.100.111 == command.101.120;",
        json!({"501":"CommittedFailureEffects"}),
    );
    assert_eq!(
        checked(&p, &r, &order(&["pre.100.111"], 0)).verdict(),
        Verdict::Refuted
    );
}
