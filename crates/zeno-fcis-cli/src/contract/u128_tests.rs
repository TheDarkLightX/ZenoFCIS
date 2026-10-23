//! Typed core commands and consumer binding witnesses, including high-bit finite domains.
use super::*;
use serde_json::{Value, json};
const H: u128 = 1u128 << 127;
fn project(min: u128, max: u128) -> String {
    format!(
        "zeno 1; project 1 unsigned_review; namespace 2 core; type 100 state State; type 101 command Command; type 102 context Context; type 105 u128 Amount in {min}..={max}; field 110 100 result 105; field 120 101 amount 105; reason 300 Cancel precedence 0; component 400 core {{ owns 100; reads pre.100; writes post.100; contexts context.102; budget steps 4096; }} merge [400]; law 500 bounded on commit, genesis = post.100.110 >= u128({min}); law 501 reject_false on reject = false;"
    )
}
fn rules(min: u128) -> String {
    json!({"schema":"zeno-fcis/template-declarative-policy/4","template":"unsigned-review","roots":{"state":100,"command":101,"context":102},"leaf_bindings":{"102":["Bool"]},"variables":{},"cases":[{"when":false,"class":"Reject","reason":300,"post":{},"outbox":[]},{"when":true,"class":"Accept","reason":null,"post":{"110":"command.101.120"},"outbox":[]}],"genesis":{"110":format!("u128({min})")},"law_kinds":{"500":"StateInvariant","501":"FeeAndRounding"}}).to_string()
}
fn sources<'a>(p: &'a str, r: &'a str) -> ContractSources<'a> {
    ContractSources {
        project: p,
        rules: r,
        schema_origin: None,
        adoptions: &[],
        replayed: &[],
        evolutions: &[],
    }
}
fn review_sources<'a>(p: &'a str, r: &'a str, e: Option<&'a str>) -> review::ReviewSources<'a> {
    review::ReviewSources {
        project: p,
        rules: r,
        examples: e,
    }
}
#[test]
fn u128_generate_and_review_keep_high_bit_values() {
    let p = project(H, H + 1);
    let r = rules(H);
    let g = generate_contract(sources(&p, &r))
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    assert!(g.program().is_none());
    assert!(g.source().contains("NumericDescriptor"));
    let e = format!("{H} {} 0 | accept - {} | -\n", H + 1, H + 1);
    let one = review::review(review_sources(&p, &r, Some(&e)), 100)
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    let two = review::review(review_sources(&p, &r, Some(&e)), 100)
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    assert_eq!(one.packet(), two.packet());
    let packet: Value = serde_json::from_str(one.packet())
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    assert_eq!(packet["summary"]["examples"]["disagreements"], 0);
    assert_eq!(packet["summary"]["inputs"]["count"], 8);
    assert!(one.packet().contains(&(H + 1).to_string()));
}
#[test]
fn u128_finite_gaps_proposals_and_limits() {
    let p = project(H, H + 1);
    let r = rules(H);
    let settings = review::gaps::Options {
        mutants: true,
        max_tuples: 100,
        max_work: 100000,
        proposals: None,
    };
    let run = |options| {
        review::review_with_gaps(review_sources(&p, &r, None), options)
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
    };
    let baseline = run(settings);
    let packet: Value = serde_json::from_str(baseline.packet())
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    assert_eq!(packet["gaps"]["admissible"]["full"]["undetermined"], 8);
    let proposal = "law 600 exact on commit = post.100.110 == command.101.120;";
    let more = run(review::gaps::Options {
        proposals: Some(proposal),
        ..settings
    });
    let packet: Value = serde_json::from_str(more.packet())
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    assert_eq!(
        packet["gaps"]["proposals"]["after"]["admissible"]["full"]["determined"],
        8
    );
    let alarm = run(review::gaps::Options {
        proposals: Some("law 600 false_alarm on commit = post.100.110 == u128(0);"),
        ..settings
    });
    assert!(alarm.packet().contains("proposal-false-alarm"));
    let over = run(review::gaps::Options {
        max_work: 1,
        ..settings
    });
    assert!(over.packet().contains("work-exceeds-limit"));
}

fn genesis_fixture(name: &str) -> (String, String) {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/test-data/genesis-proposals")
        .join(name);
    let read = |file| std::fs::read_to_string(path.join(file)).unwrap_or_else(|e| panic!("{e}"));
    (read("project.zeno"), read("v2/policy.json"))
}

// A fixed, unsigned state field selects the numeric lane without changing the
// signed witness's transitions, control labels, or genesis obligations.
fn unsigned_genesis_twin(name: &str) -> (String, String) {
    let (mut p, r) = genesis_fixture(name);
    p.push_str("\ntype 108 u128 UnsignedUnit in 0..=0;\nfield 180 100 unsigned_unit 108;\n");
    let mut r: Value = serde_json::from_str(&r).unwrap_or_else(|e| panic!("{e}"));
    r["schema"] = json!("zeno-fcis/template-declarative-policy/4");
    r["genesis"]["180"] = json!("u128(0)");
    (p, r.to_string())
}

fn genesis_review(p: &str, r: &str, formula: &str, genesis: bool) -> Value {
    let scope = if genesis { "commit, genesis" } else { "commit" };
    let proposal = format!("law 9000 p on {scope} = {formula};");
    let result = review::review_with_gaps(
        review_sources(p, r, None),
        review::gaps::Options {
            mutants: true,
            max_tuples: 1000,
            max_work: 1000000,
            proposals: Some(&proposal),
        },
    )
    .unwrap_or_else(|e| panic!("{e}"));
    serde_json::from_str(result.packet()).unwrap_or_else(|e| panic!("{e}"))
}

fn assert_unsigned_genesis_alarm(p: &str, r: &str, formula: &str, state: Value) {
    let packet = genesis_review(p, r, formula, true);
    assert_eq!(
        packet["gaps"]["status"], "proposal-false-alarm",
        "unsigned_genesis_scoped_proposal_requires_initiation"
    );
    assert_eq!(
        packet["gaps"]["false_alarm"],
        json!({"law":9000,"at":"genesis","state":state})
    );
    assert_eq!(packet["gaps"]["proposals"][0]["active"], 0);
    assert!(packet["gaps"]["proposals"].get("after").is_none());
    let review::lemma::Lemmas::Judged(result) =
        review::lemma::judge(p, r, &[formula.into()], 1000).unwrap_or_else(|e| panic!("{e}"))
    else {
        panic!("finite genesis witness")
    };
    assert_eq!(result[0]["initiation"], false);
}

#[test]
fn u128_genesis_proposals_reject_zero_to_one() {
    let (p, r) = genesis_fixture("u128-zero-one");
    assert_unsigned_genesis_alarm(
        &p,
        &r,
        "post.100.110 == u128(1)",
        json!([{"kind":"u128","value":"0"}]),
    );
}

#[test]
fn numeric_i128_genesis_proposals_require_initiation() {
    let (p, r) = genesis_fixture("u128-zero-one");
    let p = p
        .replace("type 105 u128", "type 105 int")
        .replace("u128(0)", "0");
    let mut r: Value = serde_json::from_str(&r).unwrap_or_else(|e| panic!("{e}"));
    r["genesis"]["110"] = json!(0);
    r["cases"][1]["post"]["110"] = json!("1");
    assert_unsigned_genesis_alarm(
        &p,
        &r.to_string(),
        "post.100.110 == 1",
        json!([{"kind":"i128","value":"0"}]),
    );
}

#[test]
fn u128_genesis_proposals_reject_false_without_commits() {
    let (p, r) = unsigned_genesis_twin("no-commits");
    assert_unsigned_genesis_alarm(
        &p,
        &r,
        "false",
        json!([{"kind":"i128","value":"150"},{"kind":"u128","value":"0"}]),
    );
}

#[test]
fn u128_genesis_proposals_reject_running_at_idle_genesis() {
    let (p, r) = unsigned_genesis_twin("start-once");
    assert_unsigned_genesis_alarm(
        &p,
        &r,
        "post.100.110 == 151",
        json!([{"kind":"i128","value":"150"},{"kind":"u128","value":"0"}]),
    );
}

#[test]
fn u128_genesis_proposals_reject_shipped_running_invariant() {
    let (p, r) = unsigned_genesis_twin("context-free");
    assert_unsigned_genesis_alarm(
        &p,
        &r,
        "post.100.110 == 151",
        json!([{"kind":"i128","value":"150"},{"kind":"i128","value":"0"},{"kind":"u128","value":"0"}]),
    );
}

#[test]
fn u128_genesis_proposals_reject_undefined_at_genesis() {
    let (p, r) = unsigned_genesis_twin("context-free");
    let packet = genesis_review(&p, &r, "div_floor(1, post.100.170) == 1", true);
    assert_eq!(packet["gaps"]["status"], "proposal-false-alarm");
    assert_eq!(packet["gaps"]["false_alarm"]["at"], "genesis");
    assert_eq!(packet["gaps"]["proposals"][0]["active"], 0);
}

#[test]
fn u128_genesis_proposals_accept_true_invariant() {
    let (p, r) = genesis_fixture("u128-zero-one");
    let packet = genesis_review(&p, &r, "post.100.110 <= u128(1)", true);
    assert_eq!(packet["gaps"]["status"], "measured");
    // Genesis is checked, but never added to the eight transition activations.
    assert_eq!(packet["gaps"]["proposals"]["laws"][0]["active"], 8);
    assert!(packet["gaps"]["proposals"]["after"].is_object());
    let packet = genesis_review(
        &project(H, H + 1),
        &rules(H),
        &format!("post.100.110 >= u128({H})"),
        true,
    );
    assert_eq!(packet["gaps"]["status"], "measured");
    assert_eq!(packet["gaps"]["proposals"]["laws"][0]["active"], 8);
}

#[test]
fn u128_genesis_proposals_leave_transition_only_scope_unchanged() {
    let (p, r) = genesis_fixture("u128-zero-one");
    let packet = genesis_review(&p, &r, "post.100.110 == u128(1)", false);
    assert_eq!(packet["gaps"]["status"], "measured");
    assert_eq!(packet["gaps"]["proposals"]["laws"][0]["active"], 8);
    let (p, r) = unsigned_genesis_twin("no-commits");
    let packet = genesis_review(&p, &r, "false", false);
    assert_eq!(packet["gaps"]["status"], "measured");
    assert_eq!(packet["gaps"]["proposals"]["laws"][0]["active"], 0);
}

#[test]
fn u128_lemma_state_transition_and_inconclusive() {
    let p = project(H, H + 1);
    let r = rules(H);
    let formulas = [
        format!("post.100.110 >= u128({H})"),
        "post.100.110 == u128(0)".into(),
    ];
    let review::lemma::Lemmas::Judged(result) = review::lemma::judge(&p, &r, &formulas, 100)
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
    else {
        panic!("finite domain")
    };
    assert_eq!(result[0]["verdict"], "inductive");
    assert_eq!(result[1]["verdict"], "false");
    let review::lemma::Lemmas::Judged(result) =
        review::lemma::judge_transitions(&p, &r, &["post.100.110 == command.101.120".into()], 100)
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
    else {
        panic!("finite domain")
    };
    assert_eq!(result[0]["verdict"], "checked");
    assert!(matches!(
        review::lemma::judge(&project(0, u128::MAX), &rules(0), &formulas, 100)
            .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
        review::lemma::Lemmas::Inconclusive(_)
    ));
}
#[test]
fn u128_tau_and_signed_program_binding_refuse_even_low_values() {
    for (min, max) in [(0, 1), (H, H + 1)] {
        let p = project(min, max);
        let r = rules(min);
        let err = tau::export(&p, &r, 100, 1).map_or_else(
            |error| error,
            |value| panic!("expected refusal, got {value:?}"),
        );
        assert_eq!(err.reason(), "U128SubjectUnsupported");
        let parsed =
            Rules::read(&r).unwrap_or_else(|error| panic!("required result failed: {error:?}"));
        let d = Declarations::read(&p, &parsed.leaf_bindings)
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
        let bytes =
            schema::encode(&d).unwrap_or_else(|error| panic!("required result failed: {error:?}"));
        let c = Contract::build(
            &d,
            &parsed,
            schema_commitment(&bytes)
                .unwrap_or_else(|error| panic!("required result failed: {error:?}")),
        )
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
        assert_eq!(
            c.program()
                .map_or_else(
                    |error| error,
                    |value| panic!("expected refusal, got {value:?}")
                )
                .reason(),
            "U128SubjectUnsupported"
        );
        assert_eq!(
            review::domain::positions(&d)
                .map_or_else(
                    |error| error,
                    |value| panic!("expected refusal, got {value:?}")
                )
                .reason(),
            "U128SubjectUnsupported"
        );
    }
}
#[test]
fn u128_review_preserves_existing_sum_variants() {
    let p = project(H, H + 1)
        + " type 109 data Mode; variant 130 109 First none; variant 131 109 Second none; field 121 101 mode 109;";
    let r = rules(H);
    let result = review::review(review_sources(&p, &r, None), 100)
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    let packet: Value = serde_json::from_str(result.packet())
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    assert_eq!(packet["summary"]["inputs"]["count"], 16);
    assert_eq!(packet["domain_size"], "16");
}
#[test]
fn u128_symbolic_refuses_before_solver_and_sqlite_migration_before_mapping() {
    for min in [0, H] {
        let p = project(min, min + 1);
        let r = rules(min);
        let mut solve =
            |_: &crate::symbolic::verdict::Query<'_>| panic!("unsigned subject reached a solver");
        let error = symbolic::check(
            symbolic::SymbolicSources {
                project: &p,
                rules: &r,
                strengthening: None,
            },
            100,
            &mut solve,
        )
        .err()
        .unwrap_or_else(|| panic!("required value missing"));
        assert_eq!(error.reason(), "U128SubjectUnsupported");
        let r = Rules::read(&r).unwrap_or_else(|error| panic!("required result failed: {error:?}"));
        let d = Declarations::read(&p, &r.leaf_bindings)
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
        let file = rules::read_migration(
            r#"{"schema":"zeno-fcis/migration/1","state":{"110":{"from":110}}}"#,
            "migration",
        )
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
        let e = migration::compile(&file, &d, &d, "migration")
            .err()
            .unwrap_or_else(|| panic!("required value missing"));
        assert_eq!(e.reason(), "U128SubjectUnsupported");
    }
}
#[test]
fn u128_search_optimizer_and_loop_doors_share_a_refusing_signed_json_binding() {
    use crate::neural_loop::program_json::{ProgramJsonError, program_from_json};
    for max in [1u128, u128::MAX] {
        let subject = json!({"inputs":[{"kind":"U128","min":"0","max":max.to_string()}],"outputs":[{"kind":"Int","min":"0","max":"1"}],"nodes":[["Int","0"]],"roots":[0]});
        assert_eq!(
            program_from_json(&subject).map_or_else(
                |error| error,
                |value| panic!("expected refusal, got {value:?}")
            ),
            ProgramJsonError::U128SubjectUnsupported
        );
    }
}
#[test]
fn u128_new_scaffolds_a_native_typed_application() {
    use std::path::Path;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|error| panic!("fixture time: {error:?}"))
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "zeno-fcis-u128-scaffold-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir(&root)
        .unwrap_or_else(|error| panic!("create exclusive fixture: {error:?}"));
    let input = root.join("input");
    let app = root.join("app");
    std::fs::create_dir_all(input.join("v2"))
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    std::fs::create_dir_all(input.join("tests"))
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    std::fs::create_dir_all(&app)
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    let project = project(H, H + 1).replace(" law 501 reject_false on reject = false;", "")
        + " type 108 bool Flag; field 111 100 flag 108; type 109 data Mode; variant 130 109 First none; variant 131 109 Second none; field 121 101 mode 109;";
    let mut rules: Value = serde_json::from_str(&rules(H))
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    rules["cases"][0]["when"] = json!("context.102");
    rules["leaf_bindings"]["108"] = json!(["Bool"]);
    rules["genesis"]["111"] = json!(false);
    rules["cases"][1]["post"]["111"] = json!(format!(
        "choose(context.102, false, products_ge(command.101.120, u128({}), command.101.120, u128({})))",
        u128::MAX,
        u128::MAX
    ));
    rules["cases"][1]["post"]["110"] = json!(format!(
        "mul_div_floor(command.101.120, u128({}), u128({})) + to_u128(to_i128(u128(0)))",
        u128::MAX,
        u128::MAX
    ));
    rules["law_kinds"]
        .as_object_mut()
        .unwrap_or_else(|| panic!("required value missing"))
        .remove("501");
    std::fs::write(input.join("project.zeno"), project)
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    std::fs::write(input.join("v2/policy.json"), rules.to_string())
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    std::fs::write(
        input.join("tests/decision-examples.txt"),
        format!(
            "{H} 0 {} 131 0 | accept - {} 1 | -\n{H} 1 {} 130 1 | reject 300 {H} 1 | -\n",
            H + 1,
            H + 1,
            H + 1
        ),
    )
    .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    let tree = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| panic!("required value missing"))
        .parent()
        .unwrap_or_else(|| panic!("required value missing"));
    assert_eq!(
        crate::contract_files::scaffold(&app, &input, Some(tree)),
        crate::OK
    );
    assert!(
        std::fs::read_to_string(app.join("src/lib.rs"))
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
            .contains("NumericDescriptor")
    );
    std::fs::remove_dir_all(&root)
        .unwrap_or_else(|error| panic!("remove completed fixture: {error:?}"));
}
#[test]
fn u128_reject_examples_require_the_unchanged_pre_state() {
    let p = project(H, H + 1).replace(" law 501 reject_false on reject = false;", "");
    let mut r: Value = serde_json::from_str(&rules(H))
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    r["cases"][0]["when"] = json!("context.102");
    r["law_kinds"]
        .as_object_mut()
        .unwrap_or_else(|| panic!("required value missing"))
        .remove("501");
    let r = r.to_string();
    for (post, disagrees) in [(H, 0), (H + 1, 1)] {
        let examples = format!("{H} {} 1 | reject 300 {post} | -\n", H + 1);
        let report = review::review(review_sources(&p, &r, Some(&examples)), 100)
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
        let packet: Value = serde_json::from_str(report.packet())
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
        assert_eq!(packet["summary"]["examples"]["disagreements"], disagrees);
    }
}
#[test]
fn u128_law_refusal_findings_keep_full_width_human_witnesses() {
    let p = project(H, H + 1);
    let mut r: Value = serde_json::from_str(&rules(H))
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    r["cases"][0]["when"] = json!("context.102");
    let r = r.to_string();
    let result = review::review(review_sources(&p, &r, None), 100)
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    let lines = result.law_refusal_lines();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("4 inputs"));
    assert!(lines[0].contains(&H.to_string()));
    let packet: Value = serde_json::from_str(result.packet())
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    assert_eq!(
        packet["law_refusals"][0]["input"][0]["value"],
        H.to_string()
    );
    assert_eq!(packet["summary"]["law_refusal_findings"], 1);
}

#[test]
fn u128_policy4_signed_only_build_checks_unused_bindings() {
    let p = project(0, 1)
        .replace("type 105 u128", "type 105 int")
        .replace(">= u128(0)", ">= 0");
    let mut r: Value = serde_json::from_str(&rules(0))
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    r["genesis"]["110"] = json!(0);
    for expression in ["choose(true, u128(0), 1)", "to_u128(context.102)"] {
        r["variables"] = json!({"bad":expression});
        let text = r.to_string();
        let error = generate_contract(sources(&p, &text)).map_or_else(
            |error| error,
            |value| panic!("expected refusal, got {value:?}"),
        );
        assert!(
            error.reason().contains("KindMismatch"),
            "{expression}: {error}"
        );
    }
    r["variables"] = json!({});
    let text = r.to_string();
    let generated = generate_contract(sources(&p, &text))
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
    assert!(generated.program().is_none());
    assert!(generated.source().contains("NumericDescriptor"));
    assert!(review::review(review_sources(&p, &text, None), 100).is_ok());
}

#[test]
fn numeric_count_probe_observes_original_decisions_exactly() {
    use review::probe::{Answer, Question};
    let p = project(H, H + 1);
    let r = rules(H);
    for (condition, expected) in [
        (format!("command.101.120 == u128({H})"), 4usize),
        ("u128(0) == u128(1)".into(), 0),
        ("post.100.110 == command.101.120".into(), 8),
    ] {
        let Answer::Answered(v) = review::probe::probe(&p, &r, &Question::Count { condition }, 100)
            .unwrap_or_else(|e| panic!("{e:?}"))
        else {
            panic!("exact finite query");
        };
        assert_eq!(v["answer"], expected);
        assert_eq!(v["fails"], 8 - expected);
        assert_eq!(v["undefined"], 0);
        assert_eq!(v["refused"], 0);
        assert_eq!(v["of"], 8);
    }
    assert!(matches!(
        review::probe::probe(
            &p,
            &r,
            &Question::Count {
                condition: "true".into()
            },
            4
        )
        .unwrap_or_else(|e| panic!("{e:?}")),
        Answer::Inconclusive(_)
    ));
    assert!(
        review::probe::probe(
            &p,
            &r,
            &Question::Count {
                condition: "command.101.999 == u128(0)".into()
            },
            100
        )
        .is_err()
    );
}
#[test]
fn numeric_count_probe_keeps_undefined_and_refused_separate() {
    use review::probe::{Answer, Question};
    let p = project(H, H + 1);
    let r = rules(H).replacen("\"when\":false", "\"when\":true", 1);
    let condition = Question::Count {
        condition: format!("post.100.110 == u128({H})"),
    };
    let Answer::Answered(refused) =
        review::probe::probe(&p, &r, &condition, 100).unwrap_or_else(|e| panic!("{e:?}"))
    else {
        panic!("finite query");
    };
    assert_eq!(refused["refused"], 8);
    assert_eq!(refused["undefined"], 0);
    let p = p.replace(
        "law 501 reject_false on reject = false;",
        "law 501 reject_false on reject = true;",
    );
    let Answer::Answered(undefined) =
        review::probe::probe(&p, &r, &condition, 100).unwrap_or_else(|e| panic!("{e:?}"))
    else {
        panic!("finite query");
    };
    assert_eq!(undefined["undefined"], 8);
    assert_eq!(undefined["refused"], 0);
    assert_eq!(undefined["answer"], 0);
}

#[test]
fn u128_count_and_scenario_share_native_numeric_binding() {
    for min in [0, H, u128::MAX - 1] {
        let p = project(min, min + 1);
        let r = rules(min);
        let count = review::probe::probe(
            &p,
            &r,
            &review::probe::Question::Count {
                condition: format!("post.100.110 == u128({min})"),
            },
            8,
        )
        .unwrap_or_else(|error| panic!("numeric count: {error}"));
        let review::probe::Answer::Answered(count) = count else {
            panic!("count was incomplete")
        };
        assert_eq!(count["answer"], 4);
        assert_eq!(count["fails"], 4);
        assert_eq!(count["of"], 8);
        let text = format!(
            "# scenario numeric_round_trip\n{min} {} 0 | accept - {} | -\n{} {min} 1 | accept - {min} | -\n",
            min + 1,
            min + 1,
            min + 1
        );
        let result = review::scenario::replay(&p, &r, &text, "numeric-scenarios");
        assert!(
            result.is_ok(),
            "numeric scenario must bind without narrowing: {result:?}"
        );
        let result = result.unwrap_or_else(|error| panic!("asserted bound scenario: {error}"));
        assert_eq!(result.failed, 0);
        assert_eq!(result.packet["scenarios"][0]["start"], "genesis");
        assert_eq!(result.packet["scenarios"][0]["steps"], 2);
        let repeated = review::scenario::replay(&p, &r, &text, "numeric-scenarios")
            .unwrap_or_else(|error| panic!("repeat numeric scenario: {error}"));
        assert_eq!(result.packet, repeated.packet);
        let limited = review::probe::probe(
            &p,
            &r,
            &review::probe::Question::Count {
                condition: "true".into(),
            },
            7,
        )
        .unwrap_or_else(|error| panic!("limited count: {error}"));
        assert!(matches!(limited, review::probe::Answer::Inconclusive(_)));
    }
}

#[test]
fn u128_scenario_enforces_continuity_and_withholds_failed_credit() {
    let p = project(H, H + 1);
    let r = rules(H);
    let text = format!(
        "# scenario discontinuous\n{H} {} 0 | accept - {} | -\n{H} {H} 0 | accept - {H} | -\n",
        H + 1,
        H + 1
    );
    let result = review::scenario::replay(&p, &r, &text, "numeric-scenarios")
        .unwrap_or_else(|error| panic!("numeric scenario: {error}"));
    assert_eq!(result.failed, 1);
    assert_eq!(result.packet["scenarios"][0]["failure"]["step"], 2);
    assert_eq!(result.packet["mutants"]["status"], "withheld");
    assert_eq!(result.packet["reject_everything"]["status"], "withheld");
    let wrong = format!(
        "# scenario wrong_post\n{H} {} 0 | accept - {H} | -\n",
        H + 1
    );
    let result = review::scenario::replay(&p, &r, &wrong, "numeric-scenarios")
        .unwrap_or_else(|error| panic!("numeric mismatch: {error}"));
    assert_eq!(result.failed, 1);
    assert!(result.packet.to_string().contains(&(H + 1).to_string()));
    let outcome = &result.packet["scenarios"][0]["failure"]["outcome"];
    assert_eq!(outcome["class"], "Accept");
    assert_eq!(outcome["post"], json!([(H + 1).to_string()]));
    assert_eq!(outcome["deliveries"], 0);
    let p = p.replace(
        &format!("post.100.110 >= u128({H})"),
        &format!("post.100.110 > u128({H})"),
    );
    let result = review::scenario::replay(&p, &r, &wrong, "numeric-scenarios")
        .unwrap_or_else(|error| panic!("numeric genesis: {error}"));
    assert_eq!(result.failed, 1);
    assert!(
        result.packet["scenarios"][0]["failure"]["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("no admissible starting state"))
    );
}

#[test]
fn permission_word_count_and_scenarios_use_the_numeric_authority() {
    let p = include_str!("../../../../examples/contract-tables/permission-words/project.zeno");
    let r = include_str!("../../../../examples/contract-tables/permission-words/v2/policy.json");
    let examples = include_str!(
        "../../../../examples/contract-tables/permission-words/tests/decision-examples.txt"
    );
    let count = review::probe::probe(
        p,
        r,
        &review::probe::Question::Count {
            condition: "subset(3,u128(4),permissions[1])".into(),
        },
        3584,
    )
    .unwrap_or_else(|error| panic!("permission count: {error}"));
    let review::probe::Answer::Answered(count) = count else {
        panic!("permission count incomplete")
    };
    assert_eq!(count["answer"], 2240);
    assert_eq!(count["of"], 3584);
    let scenarios = examples
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .enumerate()
        .map(|(index, line)| format!("# scenario permission_word_{index}\n{line}\n"))
        .collect::<String>();
    let result = review::scenario::replay(p, r, &scenarios, "permission-scenarios")
        .unwrap_or_else(|error| panic!("permission scenario: {error}"));
    assert_eq!(result.failed, 0);
    assert_eq!(result.packet["count"], 9);
}

#[test]
fn u128_scenario_checks_full_width_deliveries_and_committed_failures() {
    let p = (project(H, H + 1)
        + " type 103 destination Target; type 104 payload Notice; field 140 104 amount 105; channel 301 notice destination 103 payload 104;")
        .replace("contexts context.102;", "contexts context.102; outbox outbox.301;");
    for committed_failure in [false, true] {
        let p = if committed_failure {
            format!(
                "{p} reason 302 Failed precedence 1; law 502 failure_post on failure = post.100.110 == command.101.120;"
            )
        } else {
            p.clone()
        };
        let mut r: Value = serde_json::from_str(&rules(H))
            .unwrap_or_else(|error| panic!("numeric policy: {error}"));
        r["leaf_bindings"]["103"] = json!(["Text", 1, 16]);
        r["cases"][1]["outbox"] = json!([{"channel":301,"ordinal":0,"idempotency_ordinal":0,
            "destination":"target","payload":{"140":"command.101.120"}}]);
        let decision = if committed_failure {
            r["cases"][1]["class"] = json!("CommittedFailure");
            r["cases"][1]["reason"] = json!(302);
            r["law_kinds"]["502"] = json!("CommittedFailureEffects");
            "failure 302"
        } else {
            "accept -"
        };
        let r = r.to_string();
        let text = format!(
            "# scenario numeric_delivery\n{H} {} 0 | {decision} {} | 301 {}\n",
            H + 1,
            H + 1,
            H + 1
        );
        let result = review::scenario::replay(&p, &r, &text, "numeric-deliveries")
            .unwrap_or_else(|error| panic!("numeric delivery: {error}"));
        assert_eq!(result.failed, 0);
        let wrong = text.replace(&format!("301 {}", H + 1), &format!("301 {H}"));
        let result = review::scenario::replay(&p, &r, &wrong, "numeric-deliveries")
            .unwrap_or_else(|error| panic!("numeric delivery mismatch: {error}"));
        assert_eq!(result.failed, 1);
        assert_eq!(result.packet["mutants"]["status"], "withheld");
    }
}
